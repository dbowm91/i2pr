//! Plan 290 Milestone 10 SOCKS + IRC filter composition executor.
//!
//! Java I2PTunnel's "SOCKS IRC" client mode is a SOCKS proxy whose
//! post-CONNECT traffic passes through the IRC client command
//! whitelist. This module composes the two existing bounded pieces
//! with no raw bypass mode:
//!
//! ```text
//! loopback TCP accept
//!   -> shared SOCKS5/4a negotiation (version peek + CONNECT)
//!   -> SOCKS CONNECT port policy + destination resolution
//!   -> I2P Streaming connect + bounded Established wait
//!   -> version-appropriate success reply
//!   -> shared IRC client filter loop (all bytes filtered)
//! ```
//!
//! After the success reply every byte in both directions flows
//! through `run_irc_filtered_loop`; there is no opaque-pump path.
//! Same-read post-CONNECT bytes feed the filter loop as initial
//! inbound bytes so no filtered byte is lost at the handoff. The
//! profile is loopback-only, disabled by default, and introduces
//! no new Garlic/I2NP/Streaming implementation.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::sync::atomic::Ordering;

use i2pr_runtime::CancellationToken;
use i2pr_service_tunnels::{IrcClientOptions, Socks5ClientOptions, Socks5Limits};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tracing::{debug, warn};

use crate::service_tunnels::{ServiceRuntime, ServiceTunnelManager};
use crate::service_tunnels_irc_client::{IrcConnectionOutcome, run_irc_filtered_loop};
use crate::service_tunnels_socks5::{
    lookup_connect_timeout, negotiate_socks_destination, open_streaming,
    resolve_target_for_service, terminate_streaming, wait_for_established,
};

/// Outcome of one SOCKS IRC connection attempt (typed for tests).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SocksIrcConnectionOutcome {
    /// CONNECT + filter loop completed (peer closed or local
    /// cancel) and Streaming was torn down.
    TunnelClosed,
    /// Version peek, greeting, or method negotiation failed.
    BadGreeting,
    /// CONNECT request was malformed.
    BadRequest,
    /// CONNECT target rejected by policy (clearnet, port policy,
    /// unresolvable destination).
    Forbidden,
    /// Streaming connect or establishment failed.
    BadGateway,
    /// Proxy authentication failed (or was required but missing).
    AuthFailed,
    /// The connect deadline expired.
    TimedOut,
    /// The IRC filter loop observed a structural violation.
    FilterViolation,
}

/// Per-connection SOCKS IRC entry. Negotiates SOCKS, opens
/// Streaming, then runs every byte through the IRC client filter.
pub async fn run_socks_irc_connection(
    manager: Arc<ServiceTunnelManager>,
    runtime: Arc<ServiceRuntime>,
    stream: TcpStream,
    cancellation: CancellationToken,
    socks_options: Socks5ClientOptions,
    irc_options: IrcClientOptions,
) -> SocksIrcConnectionOutcome {
    use crate::service_tunnels_socks5::Socks5ConnectionOutcome;
    let limits = Socks5Limits::defaults();
    let mut stream = stream;
    let negotiation =
        // Plan 342: `socks-irc` is refused the seven-field outproxy block by
        // `kind_accepts_outproxy_block`, so its parser stays `.i2p`-only. The
        // explicit argument records that as a decision rather than an omission.
        match negotiate_socks_destination(
            &mut stream,
            limits,
            socks_options.proxy_auth.as_ref(),
            i2pr_service_tunnels::target_policy::TargetPolicy::I2pOnly,
        )
            .await
        {
            Ok(value) => value,
            Err(Socks5ConnectionOutcome::BadGreeting) => {
                return SocksIrcConnectionOutcome::BadGreeting;
            }
            Err(Socks5ConnectionOutcome::BadRequest) => {
                return SocksIrcConnectionOutcome::BadRequest;
            }
            Err(Socks5ConnectionOutcome::Forbidden) => {
                return SocksIrcConnectionOutcome::Forbidden;
            }
            Err(Socks5ConnectionOutcome::AuthFailed) => {
                return SocksIrcConnectionOutcome::AuthFailed;
            }
            Err(_) => return SocksIrcConnectionOutcome::BadGateway,
        };
    let destination = negotiation.destination;
    let leftover = negotiation.leftover;
    let via_socks4a = negotiation.via_socks4a;
    if !socks_options
        .port_policy
        .allows_connect_port(destination.port)
    {
        if via_socks4a {
            let _ = stream
                .write_all(&i2pr_service_tunnels::build_socks4a_reply(false))
                .await;
        } else {
            let reply = i2pr_service_tunnels::build_socks5_reply(
                i2pr_service_tunnels::Socks5ReplyCode::ConnectionNotAllowed,
            );
            let _ = stream.write_all(&reply).await;
        }
        let _ = stream.shutdown().await;
        return SocksIrcConnectionOutcome::Forbidden;
    }
    let target = match resolve_target_for_service(&manager, runtime.destination_id, &destination) {
        Ok(value) => value,
        Err(_error) => {
            if via_socks4a {
                let _ = stream
                    .write_all(&i2pr_service_tunnels::build_socks4a_reply(false))
                    .await;
            } else {
                let reply = i2pr_service_tunnels::build_socks5_reply(
                    i2pr_service_tunnels::Socks5ReplyCode::HostUnreachable,
                );
                let _ = stream.write_all(&reply).await;
            }
            let _ = stream.shutdown().await;
            return SocksIrcConnectionOutcome::Forbidden;
        }
    };
    let connect_timeout_ms = lookup_connect_timeout(&manager, &runtime.spec_id);
    let connection_id = match open_streaming(
        &manager,
        &runtime.spec_id,
        runtime.destination_id,
        &target.remote,
        connect_timeout_ms,
        &cancellation,
    )
    .await
    {
        Ok(id) => id,
        Err(_error) => {
            if via_socks4a {
                let _ = stream
                    .write_all(&i2pr_service_tunnels::build_socks4a_reply(false))
                    .await;
            } else {
                let reply = i2pr_service_tunnels::build_socks5_reply(
                    i2pr_service_tunnels::Socks5ReplyCode::ConnectionRefused,
                );
                let _ = stream.write_all(&reply).await;
            }
            let _ = stream.shutdown().await;
            return SocksIrcConnectionOutcome::BadGateway;
        }
    };
    if wait_for_established(
        &manager,
        runtime.destination_id,
        connection_id,
        connect_timeout_ms,
    )
    .await
    .is_err()
    {
        terminate_streaming(
            &manager,
            runtime.destination_id,
            connection_id,
            &target.remote,
            true,
        );
        let _ = stream.shutdown().await;
        return SocksIrcConnectionOutcome::TimedOut;
    }
    if via_socks4a {
        if stream
            .write_all(&i2pr_service_tunnels::build_socks4a_reply(true))
            .await
            .is_err()
        {
            terminate_streaming(
                &manager,
                runtime.destination_id,
                connection_id,
                &target.remote,
                true,
            );
            return SocksIrcConnectionOutcome::BadRequest;
        }
    } else {
        let success = i2pr_service_tunnels::build_socks5_reply(
            i2pr_service_tunnels::Socks5ReplyCode::Success,
        );
        if stream.write_all(&success).await.is_err() {
            terminate_streaming(
                &manager,
                runtime.destination_id,
                connection_id,
                &target.remote,
                true,
            );
            return SocksIrcConnectionOutcome::BadRequest;
        }
    }
    if let Err(error) = stream.flush().await {
        debug!(error = %error, "socks-irc success reply flush failed");
        terminate_streaming(
            &manager,
            runtime.destination_id,
            connection_id,
            &target.remote,
            true,
        );
        return SocksIrcConnectionOutcome::BadRequest;
    }
    // No raw bypass: every post-CONNECT byte passes the IRC
    // client filter in both directions.
    let endpoint: Arc<dyn crate::destination_streaming::StreamPumpEndpoint> =
        Arc::new(crate::service_tunnels::ServicePumpEndpoint::new_client(
            Arc::clone(&manager),
            runtime.destination_id,
            connection_id,
            target.remote.clone(),
        ));
    let outcome =
        run_irc_filtered_loop(stream, endpoint, &irc_options, &cancellation, leftover).await;
    let reset = !matches!(outcome, IrcConnectionOutcome::TunnelClosed);
    terminate_streaming(
        &manager,
        runtime.destination_id,
        connection_id,
        &target.remote,
        reset,
    );
    match outcome {
        IrcConnectionOutcome::TunnelClosed => SocksIrcConnectionOutcome::TunnelClosed,
        IrcConnectionOutcome::StructuralFailure => SocksIrcConnectionOutcome::FilterViolation,
        IrcConnectionOutcome::BadGateway => SocksIrcConnectionOutcome::BadGateway,
        IrcConnectionOutcome::TimedOut => SocksIrcConnectionOutcome::TimedOut,
    }
}

/// Runs the per-service supervisor loop for a SOCKS IRC tunnel.
pub async fn run_socks_irc_loop(
    manager: &Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    spec: &i2pr_service_tunnels::ServiceTunnelSpec,
    cancellation: &CancellationToken,
) -> Result<(), crate::service_tunnels::ServiceTunnelError> {
    let listener = runtime.client_listener.as_ref().ok_or_else(|| {
        crate::service_tunnels::ServiceTunnelError::InvalidConfig(
            "socks-irc tunnel missing loopback listener".to_owned(),
        )
    })?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| crate::service_tunnels::ServiceTunnelError::Bind(error.to_string()))?;
    tracing::info!(
        service = %spec.id.as_str(),
        bind = %local_addr,
        "socks-irc tunnel bound loopback listener"
    );
    let socks_options = spec.socks5_options.clone().unwrap_or_default();
    let irc_options = spec.irc_options.clone().unwrap_or_default();
    let drain_cancel = runtime.admission_cancellation_token();
    loop {
        let accept = tokio::select! {
            biased;
            _ = cancellation.cancelled() => break,
            // Plan 289: drained runtimes stop accepting promptly (see
            // `run_client_loop`).
            _ = drain_cancel.cancelled() => break,
            accept = listener.accept() => accept,
        };
        let (stream, _peer) = match accept {
            Ok(value) => value,
            Err(error) => {
                warn!(
                    service = %spec.id.as_str(),
                    error = %error,
                    "socks-irc listener accept failed"
                );
                continue;
            }
        };
        let aggregate_permit: Option<tokio::sync::OwnedSemaphorePermit> =
            manager.aggregate_permit().try_acquire_owned().ok();
        let Some(permit_for_task) = aggregate_permit else {
            warn!(
                service = %runtime.spec_id,
                "socks-irc tunnel aggregate ceiling reached; rejecting connection"
            );
            runtime.failed_connects.fetch_add(1, Ordering::Relaxed);
            drop(stream);
            continue;
        };
        runtime.active_connections.fetch_add(1, Ordering::Relaxed);
        let manager_for_task = Arc::clone(manager);
        let runtime_for_task = Arc::clone(runtime);
        let socks_for_task = socks_options.clone();
        let irc_for_task = irc_options.clone();
        let cancellation_for_task = cancellation.clone();
        let spec_id_for_log = runtime.spec_id.clone();
        tokio::spawn(async move {
            let _permit_for_task = permit_for_task;
            let outcome = run_socks_irc_connection(
                manager_for_task,
                runtime_for_task.clone(),
                stream,
                cancellation_for_task,
                socks_for_task,
                irc_for_task,
            )
            .await;
            if matches!(
                outcome,
                SocksIrcConnectionOutcome::BadGateway | SocksIrcConnectionOutcome::Forbidden
            ) {
                runtime_for_task
                    .failed_connects
                    .fetch_add(1, Ordering::Relaxed);
            }
            runtime_for_task.connection_finished_now();
            debug!(
                service = %spec_id_for_log,
                ?outcome,
                "socks-irc connection finished"
            );
        });
    }
    Ok(())
}
