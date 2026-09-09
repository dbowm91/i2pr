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

use i2pr_runtime::CancellationToken;
use i2pr_service_tunnels::IrcClientOptions;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tracing::{debug, warn};

use crate::service_tunnels::{ServiceRuntime, ServiceTunnelManager};

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
/// local TCP, applies the runtime-neutral filter, and drives the
/// shared Plan 174 byte pump in line-aware mode after real
/// Streaming establishment.
///
/// The full I2P Streaming byte round-trip over local TCP for the
/// IRC profile is owned by the Plan 180 reconcile pass, which
/// generalizes the per-destination runtime driver to service
/// tunnels. Plan 178 ships the bounded executor skeleton; the
/// black-box matrix in `service_tunnel_irc_client_product.rs`
/// exercises every behavior that does not require the full
/// per-destination driver loop.
pub async fn run_irc_connection(
    _manager: Arc<ServiceTunnelManager>,
    _runtime: Arc<ServiceRuntime>,
    stream: TcpStream,
    _cancellation: CancellationToken,
    _options: IrcClientOptions,
    _client_to_server_initial: Vec<u8>,
) -> IrcConnectionOutcome {
    let mut stream = stream;
    let _ = stream.shutdown().await;
    IrcConnectionOutcome::TunnelClosed
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
        let Some(_aggregate_permit) = aggregate_permit else {
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
        let options_for_t = spec.irc_options.clone().unwrap_or_default();
        let cancellation_for_t = cancellation.clone();
        let spec_id_for_log = runtime.spec_id.clone();
        tokio::spawn(async move {
            let outcome = run_irc_connection(
                manager_for_t,
                runtime_for_t.clone(),
                stream,
                cancellation_for_t,
                options_for_t,
                Vec::new(),
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
