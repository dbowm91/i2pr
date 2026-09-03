//! Plan 150 — external-harness SAM listener example binary.
//!
//! This example boots the i2pr SAM 3.1 listener on an ephemeral
//! loopback port, prints the bound port as a single line of JSON
//! to stdout, and then serves the listener until the process
//! receives SIGINT / SIGTERM (or its parent closes stdin).
//!
//! It exists so that the external-client harness
//! (`tests/integration/sam/`) can drive the real i2pr SAM service
//! through plain TCP without taking a hard dependency on the
//! `i2pr_daemon` library internals.
//!
//! The example is **not** a production daemon path; it deliberately
//! skips the identity/bootstrap pipeline. Per Plan 149 the
//! `SESSION CREATE` handler self-composes its own destination
//! runtime from SAM protocol commands alone, so no router
//! identity is required.
//!
//! Usage:
//!   cargo run --example sam_loopback_listener -- --port 0
//!   ... prints `{"port":NNN,"pid":PPP}` to stdout ...

use std::sync::Arc;

use i2pr_api::sam::limits::SamLimits;
use i2pr_daemon::config::SamConfig;
use i2pr_daemon::sam::SamServiceState;
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};

fn sam_config(port: u16) -> SamConfig {
    SamConfig {
        enabled: true,
        bind_address: "127.0.0.1".parse().expect("loopback"),
        port,
        limits: SamLimits::loopback_test_profile(),
    }
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

    let state = Arc::new(SamServiceState::new(sam_config(port))?);
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

    // Print the bound port as a single JSON line and flush so the
    // harness can read it synchronously.
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

    // Block until SIGINT / SIGTERM or until the cancellation token
    // is observed as cancelled by an external signal.
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
