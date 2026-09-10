//! Plan 178 Milestone 10 IRC `.i2p` client tunnel profile executor.
//!
//! The Plan 178 IRC client tunnel is the third M10 application
//! profile that owns a bounded protocol filter. The runtime-neutral
//! policy (line bounds, IRCv3 tag framing, command allowlist,
//! USER/PING/QUIT/PART rewrites, CTCP/DCC policy) lives in
//! `i2pr-service-tunnels::irc`; this module owns the socket and
//! task orchestration.
//!
//! ```text
//! loopback TCP accept
//!   -> per-direction bounded line parser (deadline)
//!   -> runtime-neutral command classifier + privacy filter
//!   -> Manager destination resolution (Base32 / alias / local)
//!   -> I2P Streaming connect (CSPRNG, OS)
//!   -> run the Plan 174 byte pump in line-aware mode
//! ```
//!
//! The tunnel is loopback-only, disabled by default, and uses the
//! existing Plan 149 local destination product path plus the Plan
//! 174 shared socket <-> Streaming pump. No new Garlic/I2NP/Streaming
//! implementation is introduced.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use i2pr_client::streaming::connection::{ConnectionId, ConnectionState};
use i2pr_client::streaming::manager::{ConnectOutcome, DEFAULT_ADVERTISED_MAX_PAYLOAD};
use i2pr_crypto::OsRng;
use i2pr_runtime::CancellationToken;
use i2pr_service_tunnels::irc::{
    FilterOutcome, PingRewriteState, PrivacySubstitutions, classify_core, decide_client_to_server,
    decide_server_to_client,
};
use i2pr_service_tunnels::{IrcClientOptions, IrcLimits, ServiceTunnelSpec};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{debug, warn};

use crate::destination_streaming::{PumpEndpointError, PumpSendDisposition, StreamPumpEndpoint};
use crate::service_tunnels::{
    ServicePumpEndpoint, ServiceRuntime, ServiceTunnelManager, service_streaming_now_ms,
};

/// Default ceiling for the per-direction partial-line read. The
/// parser itself enforces the per-line size cap; this deadline
/// bounds stalls when no CRLF has arrived yet.
#[allow(dead_code)]
const IRC_LINE_READ_DEADLINE: Duration = Duration::from_secs(30);

/// Outcome of one IRC connection attempt (typed for tests).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrcConnectionOutcome {
    /// Tunnel completed (peer closed or local cancel) and
    /// Streaming was torn down.
    TunnelClosed,
    /// A structural failure was observed (overlong line, invalid
    /// tag framing, control byte, etc) and the socket was closed.
    StructuralFailure,
    /// Destination resolution or Streaming connect failed.
    BadGateway,
    /// A line read or connect deadline expired.
    TimedOut,
}

/// Per-connection IRC client entry. Reads IRC lines from the
/// local TCP, applies the runtime-neutral filter, and drives
/// Streaming admission in line-aware mode after real Streaming
/// establishment.
///
/// Plan 182 completes the Plan 178 executor skeleton: resolve the
/// configured destination, connect `(0, 0)` per the SAM convention,
/// wait bounded for `Established`, then run the bounded
/// line-oriented filter loop in both directions (client-to-server
/// privacy rewrites via `decide_client_to_server`, server-to-client
/// PING rewrite via `decide_server_to_client`). The shared opaque
/// `run_stream_pump` is untouched: this profile needs line framing
/// before admission, so it owns a narrow line driver instead.
pub async fn run_irc_connection(
    manager: Arc<ServiceTunnelManager>,
    runtime: Arc<ServiceRuntime>,
    spec: &ServiceTunnelSpec,
    stream: TcpStream,
    cancellation: CancellationToken,
    options: IrcClientOptions,
) -> IrcConnectionOutcome {
    let target = match manager.resolve_client_destination(spec) {
        Ok(target) => target,
        Err(error) => {
            debug!(service = %runtime.spec_id, error = %error, "irc resolve failed");
            return IrcConnectionOutcome::BadGateway;
        }
    };
    let identity_arc =
        match manager.with_destination_bridge(runtime.destination_id, |bridge| bridge.identity()) {
            Some(identity) => identity,
            None => return IrcConnectionOutcome::BadGateway,
        };
    let connect_outcome = {
        let mut os_rng = OsRng;
        let mut rng = rand_core::UnwrapMut(&mut os_rng);
        manager.with_destination_bridge(runtime.destination_id, |bridge| {
            bridge.streaming_mut().connect(
                identity_arc.as_ref(),
                &target.remote,
                0,
                0,
                DEFAULT_ADVERTISED_MAX_PAYLOAD,
                service_streaming_now_ms(),
                &mut rng,
            )
        })
    };
    let connection_id = match connect_outcome {
        Some(Ok(ConnectOutcome::SynSent { connection_id, .. })) => connection_id,
        other => {
            debug!(service = %runtime.spec_id, outcome = ?other.is_some(), "irc connect no SYN");
            return IrcConnectionOutcome::BadGateway;
        }
    };
    // Plan 182: kick the delivery driver so the queued SYN is
    // routed immediately instead of waiting for the fallback tick.
    manager.notify_outbound_signal(runtime.destination_id);
    if wait_for_established(&manager, runtime.destination_id, connection_id, spec).await {
        debug!(service = %runtime.spec_id, connection_id = connection_id.raw(), "irc established");
        let endpoint: Arc<dyn StreamPumpEndpoint> = Arc::new(ServicePumpEndpoint::new_client(
            Arc::clone(&manager),
            runtime.destination_id,
            connection_id,
            target.remote.clone(),
        ));
        let outcome = run_irc_filtered_loop(stream, endpoint, &options, &cancellation).await;
        manager.with_destination_bridge(runtime.destination_id, |bridge| {
            let _ = bridge.streaming_mut().remove_connection(connection_id);
        });
        outcome
    } else {
        manager.with_destination_bridge(runtime.destination_id, |bridge| {
            let _ = bridge.streaming_mut().remove_connection(connection_id);
        });
        IrcConnectionOutcome::TimedOut
    }
}

/// Bounded wait for one client connection to reach `Established`.
fn wait_for_established(
    manager: &Arc<ServiceTunnelManager>,
    destination_id: i2pr_client::DestinationId,
    connection_id: ConnectionId,
    spec: &ServiceTunnelSpec,
) -> impl std::future::Future<Output = bool> + Send {
    let manager = Arc::clone(manager);
    let connect_timeout_ms = spec.timeouts.connect_timeout_ms;
    async move {
        let deadline = std::time::Instant::now() + Duration::from_millis(connect_timeout_ms.max(1));
        loop {
            let established = manager
                .with_destination_bridge(destination_id, |bridge| {
                    bridge
                        .streaming()
                        .get_connection(connection_id)
                        .is_some_and(|conn| matches!(conn.state(), ConnectionState::Established))
                })
                .unwrap_or(false);
            if established {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}

/// Bounded backpressure retries before a filtered admission times out.
const IRC_ADMIT_RETRIES: usize = 400;
/// Park between backpressured admission retries.
const IRC_ADMIT_PARK: Duration = Duration::from_millis(5);
/// TCP read poll cadence. The loop must keep draining delivered
/// bytes while waiting for the next local line, so reads poll
/// instead of blocking on the 30 s line deadline: a blocking read
/// would stall the server-to-client direction whenever the local
/// client pauses mid-conversation (the failure that stalled the
/// first Plan 182 round-trip probe).
const IRC_READ_POLL: Duration = Duration::from_millis(50);

/// Runs the bounded line-oriented filter loop for one established
/// IRC client connection. Client-to-server lines pass through
/// `decide_client_to_server`; delivered server-to-client bytes are
/// reframed and pass through `decide_server_to_client` (PING
/// rewrite state retained across the connection). Structural
/// violations close the connection; unknown/dropped lines are
/// skipped without disturbing siblings.
async fn run_irc_filtered_loop(
    stream: TcpStream,
    endpoint: Arc<dyn StreamPumpEndpoint>,
    options: &IrcClientOptions,
    cancellation: &CancellationToken,
) -> IrcConnectionOutcome {
    let limits = IrcLimits::defaults();
    let substitutions = PrivacySubstitutions::default();
    let mut ping_state = PingRewriteState::new();
    let (mut reader, mut writer) = stream.into_split();
    let mut inbound = Vec::new();
    let mut outbound = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        if cancellation.is_cancelled() || endpoint.is_terminal() {
            return IrcConnectionOutcome::TunnelClosed;
        }
        // ----- Local -> Streaming: one bounded poll read, then filter -----
        // Poll (never block): delivered server-to-client bytes must
        // keep flowing while the local client is idle.
        let read = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return IrcConnectionOutcome::TunnelClosed,
            result = timeout(IRC_READ_POLL, reader.read(&mut chunk)) => match result {
                // Orderly half-close: emit CLOSE so the peer
                // observes EOF and in-flight bytes still drain.
                Ok(Ok(0)) => {
                    endpoint.shutdown_write();
                    return IrcConnectionOutcome::TunnelClosed;
                }
                Ok(Ok(n)) => n,
                Ok(Err(_)) => return IrcConnectionOutcome::TunnelClosed,
                Err(_) => 0,
            },
        };
        inbound.extend_from_slice(&chunk[..read]);
        if inbound.len() > limits.line_buffer_max_bytes {
            return IrcConnectionOutcome::StructuralFailure;
        }
        while let Some(line) = take_crlf_line(&mut inbound) {
            if line.len() + 2 > limits.core_max_bytes {
                return IrcConnectionOutcome::StructuralFailure;
            }
            let (core, original) = split_tag_envelope(&line, &limits);
            let Some(core) = core else {
                return IrcConnectionOutcome::StructuralFailure;
            };
            let parsed = match classify_core(core) {
                Ok(parsed) => parsed,
                Err(_) => return IrcConnectionOutcome::StructuralFailure,
            };
            // Plan 182: `Allow` forwards the parsed line content,
            // which carries no terminator after CRLF framing, so
            // re-append CRLF here. `Rewrite` lines already carry
            // their terminating CRLF per the filter contract.
            // Dropping the terminator (the previous behavior) fused
            // lines and stalled the server interceptor forever.
            let emit = match decide_client_to_server(
                &parsed,
                options.reason_rewrite,
                &substitutions,
                options.user_realname_max_bytes,
            ) {
                FilterOutcome::Allow => {
                    let mut forwarded = original;
                    forwarded.extend_from_slice(b"\r\n");
                    Some(forwarded)
                }
                FilterOutcome::Rewrite { line } => Some(line),
                FilterOutcome::Drop(_) => None,
            };
            if let Some(bytes) = emit
                && let Err(outcome) = admit_bytes(endpoint.as_ref(), &bytes).await
            {
                return outcome;
            }
            endpoint.notify_outbound();
        }
        // ----- Streaming -> Local: reframe, filter, write -----
        for delivered in endpoint.drain_delivered() {
            if delivered.is_empty() {
                continue;
            }
            outbound.extend_from_slice(&delivered);
            if outbound.len() > limits.line_buffer_max_bytes {
                return IrcConnectionOutcome::StructuralFailure;
            }
            while let Some(line) = take_crlf_line(&mut outbound) {
                if line.len() + 2 > limits.core_max_bytes {
                    return IrcConnectionOutcome::StructuralFailure;
                }
                let (core, original) = split_tag_envelope(&line, &limits);
                let Some(core) = core else {
                    return IrcConnectionOutcome::StructuralFailure;
                };
                let parsed = match classify_core(core) {
                    Ok(parsed) => parsed,
                    Err(_) => return IrcConnectionOutcome::StructuralFailure,
                };
                let emit = match decide_server_to_client(&parsed, &mut ping_state) {
                    FilterOutcome::Allow => {
                        let mut forwarded = original;
                        forwarded.extend_from_slice(b"\r\n");
                        Some(forwarded)
                    }
                    FilterOutcome::Rewrite { line } => Some(line),
                    FilterOutcome::Drop(_) => None,
                };
                if let Some(bytes) = emit
                    && writer.write_all(&bytes).await.is_err()
                {
                    return IrcConnectionOutcome::TunnelClosed;
                }
            }
        }
        if writer.flush().await.is_err() {
            return IrcConnectionOutcome::TunnelClosed;
        }
        tokio::task::yield_now().await;
    }
}

/// Pops one CRLF-terminated line (without the terminator) from the
/// front of `buffer`. Returns `None` when no complete line is
/// buffered yet.
fn take_crlf_line(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    let end = buffer.windows(2).position(|window| window == b"\r\n")?;
    let line = buffer[..end].to_vec();
    buffer.drain(..end + 2);
    Some(line)
}

/// Splits an optional IRCv3 tag envelope from a core line. Returns
/// the post-envelope core plus the original full line to forward on
/// `Allow`. A malformed envelope (no separating space, overlong
/// envelope) yields `None` and the caller fails closed.
fn split_tag_envelope<'a>(line: &'a [u8], limits: &IrcLimits) -> (Option<&'a [u8]>, Vec<u8>) {
    let original = line.to_vec();
    if !line.starts_with(b"@") {
        return (Some(line), original);
    }
    let Some(space) = line.iter().position(|byte| *byte == b' ') else {
        return (None, original);
    };
    if space > limits.tag_envelope_max_bytes {
        return (None, original);
    }
    (Some(&line[space + 1..]), original)
}

/// Admits filtered bytes through the owning endpoint in
/// max-payload segments with bounded backpressure retries.
async fn admit_bytes(
    endpoint: &dyn StreamPumpEndpoint,
    bytes: &[u8],
) -> Result<(), IrcConnectionOutcome> {
    let max_payload = endpoint.max_payload_bytes().max(1);
    let mut offset = 0_usize;
    let mut retries = 0_usize;
    while offset < bytes.len() {
        let end = (offset + max_payload).min(bytes.len());
        match endpoint.try_send(&bytes[offset..end]) {
            Ok(PumpSendDisposition::Accepted) => {
                offset = end;
                retries = 0;
                endpoint.notify_outbound();
            }
            Ok(PumpSendDisposition::Backpressured) => {
                retries = retries.saturating_add(1);
                if retries > IRC_ADMIT_RETRIES {
                    return Err(IrcConnectionOutcome::TimedOut);
                }
                tokio::time::sleep(IRC_ADMIT_PARK).await;
            }
            Err(PumpEndpointError::UnknownConnection | PumpEndpointError::InvalidState) => {
                return Err(IrcConnectionOutcome::BadGateway);
            }
            Err(_) => return Err(IrcConnectionOutcome::StructuralFailure),
        }
    }
    Ok(())
}

/// Per-service supervisor loop for an IRC client tunnel. Binds
/// one loopback listener and dispatches each accept to a
/// per-connection task under the owner `ChildScope`.
pub async fn run_irc_client_loop(
    manager: &Arc<ServiceTunnelManager>,
    runtime: &Arc<ServiceRuntime>,
    spec: &i2pr_service_tunnels::ServiceTunnelSpec,
    cancellation: &CancellationToken,
) -> Result<(), crate::service_tunnels::ServiceTunnelError> {
    let listener = runtime.client_listener.as_ref().ok_or_else(|| {
        crate::service_tunnels::ServiceTunnelError::InvalidConfig(
            "irc client tunnel missing loopback listener".to_owned(),
        )
    })?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| crate::service_tunnels::ServiceTunnelError::Bind(error.to_string()))?;
    tracing::info!(
        service = %spec.id.as_str(),
        bind = %local_addr,
        "irc client tunnel bound loopback listener"
    );
    let _ = spec.irc_options.clone().unwrap_or_default();
    loop {
        let accept = tokio::select! {
            biased;
            _ = cancellation.cancelled() => break,
            accept = listener.accept() => accept,
        };
        let (stream, _peer) = match accept {
            Ok(value) => value,
            Err(error) => {
                warn!(
                    service = %spec.id.as_str(),
                    error = %error,
                    "irc client listener accept failed"
                );
                continue;
            }
        };
        let aggregate_permit: Option<tokio::sync::OwnedSemaphorePermit> =
            manager.aggregate_permit().try_acquire_owned().ok();
        let Some(permit_for_task) = aggregate_permit else {
            warn!(
                service = %runtime.spec_id,
                "irc client tunnel aggregate ceiling reached; rejecting connection"
            );
            runtime.failed_connects.fetch_add(1, Ordering::Relaxed);
            drop(stream);
            continue;
        };
        runtime.active_connections.fetch_add(1, Ordering::Relaxed);
        let manager_for_t = Arc::clone(manager);
        let runtime_for_t = Arc::clone(runtime);
        let spec_for_t = spec.clone();
        let options_for_t = spec.irc_options.clone().unwrap_or_default();
        let cancellation_for_t = cancellation.clone();
        let spec_id_for_log = runtime.spec_id.clone();
        tokio::spawn(async move {
            // Plan 182: hold the aggregate slot for the connection
            // lifetime; the previous let-else binding released it
            // at spawn time so the ceiling never engaged.
            let _permit_for_task = permit_for_task;
            let outcome = run_irc_connection(
                manager_for_t,
                runtime_for_t.clone(),
                &spec_for_t,
                stream,
                cancellation_for_t,
                options_for_t,
            )
            .await;
            if matches!(
                outcome,
                IrcConnectionOutcome::BadGateway
                    | IrcConnectionOutcome::StructuralFailure
                    | IrcConnectionOutcome::TimedOut
            ) {
                runtime_for_t
                    .failed_connects
                    .fetch_add(1, Ordering::Relaxed);
            }
            runtime_for_t
                .active_connections
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    Some(value.saturating_sub(1))
                })
                .ok();
            debug!(
                service = %spec_id_for_log,
                ?outcome,
                "irc client connection finished"
            );
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_read_deadline_is_bounded() {
        assert!(IRC_LINE_READ_DEADLINE.as_secs() >= 5);
    }
}
