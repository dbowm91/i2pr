//! Plan 170 — external-harness I2CP listener example binary.
//!
//! This example boots the i2pr I2CP 0.9.67 listener on an ephemeral
//! loopback port, prints the bound port as a single line of JSON to
//! stdout, and then serves the listener until the process receives
//! SIGINT / SIGTERM (or its parent closes stdin).
//!
//! It exists so that the external-client harness
//! (`tests/integration/i2cp/external/`) can drive the real i2pr I2CP
//! service through plain TCP without taking a hard dependency on the
//! `i2pr_daemon` library internals.
//!
//! The example is **not** a production daemon path; it deliberately
//! skips the identity/bootstrap pipeline. Per Plan 167 the listener
//! is disabled by default; this example opts in through a
//! loopback-only test profile.
//!
//! Usage:
//!   cargo run --example i2cp_loopback_listener -- [--port N]
//!   ... prints `{"port":NNN,"pid":PPP}` to stdout ...
//!
//! The default port (`0`) selects an ephemeral loopback port. The
//! harness can override it through `--port N`. The default
//! `--port 7654` path is the standard I2CP loopback port that
//! every unmodified external client (`i2pd`, Java I2P, go-i2cp)
//! connects to when only the host is configured; we keep it as the
//! explicit override so the harness does not depend on
//! implementation-specific behavior in the external library.

use std::sync::Arc;

use i2pr_daemon::config::I2cpConfig;
use i2pr_daemon::i2cp::I2cpServiceState;
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};

// M9 I2CP profile ceilings stay inside the bounded destination registry
// (`MAX_LOCAL_DESTINATIONS = 16`); cross-client trajectories open at most
// one session per connection so 16 connections is enough for the harness.
const MAX_CLIENTS: u16 = 16;
const MAX_SESSIONS_ROUTER: u16 = 16;
const MAX_BUFFERED_BYTES_PER_CONNECTION: usize = 64 * 1024;
const MAX_PENDING_WRITES_PER_CONNECTION: u32 = 64;

fn build_config(port: u16) -> I2cpConfig {
    let mut config = I2cpConfig::loopback_test_profile(
        MAX_CLIENTS,
        MAX_SESSIONS_ROUTER,
        MAX_BUFFERED_BYTES_PER_CONNECTION,
        MAX_PENDING_WRITES_PER_CONNECTION,
    );
    config.port = port;
    config
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut port: u16 = 0;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                port = args
                    .next()
                    .ok_or("missing value for --port")?
                    .parse()
                    .map_err(|_| "invalid --port value")?;
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }

    let state = Arc::new(I2cpServiceState::new(build_config(port))?);
    let bind_address = state.bind_address();
    let (listener, bound_address) = state.bind(bind_address).await?;
    let parent = CancellationToken::new();
    let scope = ChildScope::for_test(&parent, ChildFailurePolicy::FailParent);
    let task_state = Arc::clone(&state);
    let task_parent = parent.clone();
    let task_scope = scope.clone();
    let serve_scope = scope.clone();
    serve_scope
        .spawn(move |task_cancellation| {
            let _ = task_cancellation;
            async move {
                let _ = task_state.serve(listener, task_scope, task_parent).await;
                Ok(())
            }
        })
        .expect("spawn listener task");
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }

    let json = format!(
        "{{\"port\":{},\"pid\":{}}}\n",
        bound_address.port(),
        std::process::id()
    );
    {
        use std::io::Write;
        let stdout = std::io::stdout();
        let mut handle = stdout.lock();
        handle.write_all(json.as_bytes())?;
        handle.flush()?;
    }

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("install SIGTERM handler");
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .expect("install SIGINT handler");
    tokio::select! {
        _ = sigterm.recv() => {}
        _ = sigint.recv() => {}
    }

    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    Ok(())
}
