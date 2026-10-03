//! Plan 290 Milestone 10 bidirectional HTTP server executor.
//!
//! Java I2PTunnel's deprecated HTTP bidirectional mode "functions
//! as both a I2PTunnel HTTP Server, and a I2PTunnel HTTP client
//! with no outproxying capabilities". This module composes those
//! two halves under one lifecycle generation and one persistent
//! public server identity:
//!
//! - server half: accepted inbound Streaming filtered through the
//!   Plan 290 HTTP server filter to the loopback target (shared
//!   SYN accept path, shared relay);
//! - client half: the loopback HTTP proxy path (`http-client`
//!   behavior with no outproxy — i2pr's HTTP client has no
//!   outproxy capability, so the deprecation constraint holds by
//!   construction).
//!
//! Only the server half publishes: the client half originates
//! streams from the same persistent destination and resolves
//! per-request destinations like `http-client`. No second
//! destination is created, published, or stored. Deprecation does
//! not relax any bound: clearnet, non-loopback, and unsupported
//! framing stay fail-closed on both halves.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use i2pr_runtime::CancellationToken;
use tracing::{debug, warn};

use crate::service_tunnels::{
    ServiceRuntime, ServiceTunnelManager, accept_server_syn, poll_streaming_accept,
};
use crate::service_tunnels_http::{HttpConnectionOutcome, run_http_connection};
use crate::service_tunnels_http_server::{HttpServerConnectionOutcome, run_http_server_connection};

/// Runs the per-service supervisor loop for a bidirectional HTTP
/// server tunnel: one loopback client listener (proxy half) plus
/// the Streaming SYN poll (server half) under one supervisor task,
/// one generation, and one destination identity.
pub async fn run_http_bidir_loop(
    manager: &Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    spec: &i2pr_service_tunnels::ServiceTunnelSpec,
    cancellation: &CancellationToken,
) -> Result<(), crate::service_tunnels::ServiceTunnelError> {
    let listener = runtime.client_listener.as_ref().ok_or_else(|| {
        crate::service_tunnels::ServiceTunnelError::InvalidConfig(
            "http-bidir-server missing loopback listener".to_owned(),
        )
    })?;
    let target_socket = runtime.server_target.ok_or_else(|| {
        crate::service_tunnels::ServiceTunnelError::InvalidConfig(
            "http-bidir-server missing loopback target".to_owned(),
        )
    })?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| crate::service_tunnels::ServiceTunnelError::Bind(error.to_string()))?;
    tracing::info!(
        service = %spec.id.as_str(),
        bind = %local_addr,
        target = %target_socket,
        "http-bidir-server bound loopback listener and streaming target"
    );
    let http_options = spec.http_options.clone().unwrap_or_default();
    let mut ticker = tokio::time::interval(Duration::from_millis(50));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await;
    let port = runtime.server_streaming_port.unwrap_or(0_u16);
    let drain_cancel = runtime.cancellation_token();
    loop {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => break,
            // Plan 289: drained runtimes stop both halves promptly.
            _ = drain_cancel.cancelled() => break,
            accept = listener.accept() => {
                handle_bidir_client_accept(
                    manager,
                    runtime,
                    accept,
                    &http_options,
                    cancellation,
                    spec.id.as_str(),
                )
                .await;
            }
            _ = ticker.tick() => {
                for connection_id in poll_streaming_accept(manager, runtime, port) {
                    handle_bidir_server_syn(manager, runtime, connection_id, cancellation).await;
                }
            }
        }
    }
    Ok(())
}

/// Accepts one client-half TCP connection: aggregate permit,
/// active-slot accounting, then the shared HTTP proxy connection
/// task.
async fn handle_bidir_client_accept(
    manager: &Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    accept: std::io::Result<(tokio::net::TcpStream, std::net::SocketAddr)>,
    http_options: &i2pr_service_tunnels::HttpClientOptions,
    cancellation: &CancellationToken,
    spec_id: &str,
) {
    let (stream, _peer) = match accept {
        Ok(value) => value,
        Err(error) => {
            warn!(
                service = %spec_id,
                error = %error,
                "http-bidir-server client accept failed"
            );
            return;
        }
    };
    let aggregate_permit: Option<tokio::sync::OwnedSemaphorePermit> =
        manager.aggregate_permit().try_acquire_owned().ok();
    let Some(permit_for_task) = aggregate_permit else {
        warn!(
            service = %runtime.spec_id,
            "http-bidir-server aggregate ceiling reached; rejecting client connection"
        );
        runtime.failed_connects.fetch_add(1, Ordering::Relaxed);
        drop(stream);
        return;
    };
    runtime.active_connections.fetch_add(1, Ordering::Relaxed);
    let manager_for_task = Arc::clone(manager);
    let runtime_for_task = Arc::clone(runtime);
    let options_for_task = http_options.clone();
    let cancellation_for_task = cancellation.clone();
    let spec_id_for_log = runtime.spec_id.clone();
    tokio::spawn(async move {
        let _permit_for_task = permit_for_task;
        let outcome = run_http_connection(
            manager_for_task,
            runtime_for_task.clone(),
            stream,
            cancellation_for_task,
            options_for_task,
        )
        .await;
        if matches!(
            outcome,
            HttpConnectionOutcome::BadGateway | HttpConnectionOutcome::Forbidden
        ) {
            runtime_for_task
                .failed_connects
                .fetch_add(1, Ordering::Relaxed);
        }
        runtime_for_task.connection_finished_now();
        debug!(
            service = %spec_id_for_log,
            ?outcome,
            "http-bidir-server client connection finished"
        );
    });
}

/// Accepts one server-half inbound SYN: shared SYN accept, then
/// the shared filtered relay task under the aggregate slot.
async fn handle_bidir_server_syn(
    manager: &Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    connection_id: i2pr_client::streaming::connection::ConnectionId,
    cancellation: &CancellationToken,
) {
    let Some((peer, permit_for_task)) = accept_server_syn(manager, runtime, connection_id).await
    else {
        return;
    };
    let manager_for_task = Arc::clone(manager);
    let runtime_for_task = Arc::clone(runtime);
    let cancellation_for_task = cancellation.clone();
    let spec_id_for_log = runtime.spec_id.clone();
    // Plan 296: resolve the ordered dial targets per connection so
    // multihoming rotation advances once per accepted SYN.
    let dial_targets = manager.server_dial_targets_for(runtime);
    tokio::spawn(async move {
        let outcome = run_http_server_connection(
            manager_for_task,
            runtime_for_task.clone(),
            dial_targets,
            connection_id,
            peer,
            cancellation_for_task,
        )
        .await;
        if matches!(
            outcome,
            HttpServerConnectionOutcome::BadGateway | HttpServerConnectionOutcome::BadRequest
        ) {
            runtime_for_task
                .failed_connects
                .fetch_add(1, Ordering::Relaxed);
        }
        runtime_for_task.connection_finished_now();
        drop(permit_for_task);
        debug!(
            service = %spec_id_for_log,
            ?outcome,
            "http-bidir-server server connection finished"
        );
    });
}
